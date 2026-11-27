# The UI focus remount vs the Flickable wrap — Chunk Plan (#1490, #1502)

- **Date**: 2026-11-27
- **Kind**: stage-1 (planning) deliverable for the dispatch chunk "UI focus
  remount vs the Flickable wrap". Issues: #1490 (p1, ui), #1502 (p3,
  testing). Origin ruling: #1419 option A, operator, 2026-11-25
  (issue comment 7172599685).
- **Status**: READY for stage 2, **conditional on experiment E1 or E2
  going green** (see §4/§5). If both experiments fail, the honest
  fallback executes (§6 rollback): the wrap stays withdrawn and the
  evidence is recorded on #1490 — the wrap is never re-forced.
- **No code in this commit.**

---

## 1. The recorded reality at `origin/main` (c9fe5407)

### 1.1 The wrap-less board columns row

`crates/taskboard-ui-slint/ui/compositions/board-layout.slint:313` — the
columns row is a bare `HorizontalLayout` (spacing `Theme.space-3`,
padding left/right/bottom) whose children are the per-stack
`for column[column-index] in root.stacks: drop-zone := DropArea` rows
(`board-layout.slint:317`). Each column card carries the inline create
field (`if (column.create-open): TextField`, `board-layout.slint:655`)
and the per-column rename field (`board-layout.slint:466`). No clip, no
Flickable, no `content-x`. The per-column seek consumption
(`column-seek-nonce` / `FocusScrollHint` read) and the
`columns_seek_probe` pins are **withdrawn** (they lived only on the
withdrawn wrap commits).

### 1.2 The problem the wrap solves (#1419, option A ruling)

`docs/widget_interactions.org:295` carries the law text. Without a
scroll container, the columns row cannot be translated when the
resolved widths overflow (the translation law, #1419 body: a layout
under a non-layout parent has its x/width bindings overridden by the
default-geometry pass), and clipped columns paint off-edge forever.
The operator ruled option A: the row is a Flickable (`columns-row :=
Flickable { columns := HorizontalLayout … }`), clipped focus
auto-scrolls the nearest scrollable ancestor (the per-column seek), and
the refresh-synchronous focus drop inside the clipped subtree is a
**synthetic unmount echo, not a dismissal** — absorbed by the echo
window. The wrap geometry to re-land is exactly commit `91c33b23`
(revert-of-the-revert of `8ca5291c`'s withdrawal; original `fead35c2`):
`board-layout.slint` hunk + `tests/columns_seek_probe.slint` +
`tests/probes/columns_seek_probe_tests.rs` +
`tests/probe_composition.rs` registration (206 insertions).

### 1.3 The focus machinery

- **Window focus item**: slint's `WindowInner::set_focus_item`
  (vendored `i-slint-core-1.18.1/window.rs:1451`). Key dispatch
  capture-tunnels through the focused item's ancestors only — with no
  focus item, every key dead-ends.
- **The mount grab**: `ui/widgets/text-field.slint:159` —
  `init => { input.text = root.initial-text; if (root.autofocus)
  { input.focus(); } … }` plus the D20 init publish
  (`text-field.slint:171-174`).
- **The loss seam**: `text-field.slint:79` `callback focus-lost()`,
  fired from the `changed input-has-focus` handler
  (`text-field.slint:90-107`) → wired to
  `window.on_text_flow_focus_lost` in the adapter.
- **Why the release is DEFERRED — source evidence (new this round)**:
  slint `changed` handlers and the window's focus visibility tracker
  are `ChangeTracker`s, whose notify callbacks are queued and run by
  `ChangeTracker::run_change_handlers()` at the event-loop pass
  (`i-slint-core-1.18.1/properties/change_tracker.rs:16-19` doc:
  "the Change Tracker … notify callback is called when
  `Self::run_change_handlers()` is called"; driven at
  `i-slint-core-1.18.1/context.rs:221` and
  `i-slint-core-1.18.1/window.rs:805`). So the old field instance's
  `input-has-focus` change — and the visibility-tracker drop
  (`window.rs:1548-1558`, which drops focus when a clip ancestor
  reports the item invisible — the bisected clip-property-alone
  mechanism) — deliver **after** the dispatch's synchronous processing,
  i.e. after the rebuilt field's `init => input.focus()` has already
  run. The deferred release then clears the window's focus item: the
  window ends the pass with no focus item, keys dead-end, and the
  teardown fires `text_flow_focus_lost` deferred.

### 1.4 The landed absorb seam (kept, wrap-independent)

- Arm: `crates/taskboard-ui-slint/src/adapter/refresh.rs:47-48` (the
  push pass) and `adapter/refresh.rs:443` (`apply_outcomes`);
  `refresh_push_in_progress: std::cell::Cell<bool>` at
  `adapter/mod.rs:398`, initialised `adapter/mod.rs:1192`.
- Absorb: `crates/taskboard-ui-slint/src/adapter/seams/task.rs:44-67`
  — the first focus-lost of an armed window is the echo (absorbed +
  disarmed); flow-aware: a flowless loss inside the window returns
  early (no settle), an armed flow's loss settles genuinely.
- Law text: `docs/widget_interactions.org:295-330` (remount half
  explicitly NOT landed, riding #1490).
- **What the seam does NOT do**: it absorbs the dismissal echo but
  does not restore the focus item. The window still ends focusless —
  that is #1490.

### 1.5 The exact tests

- **The red test with the wrap present**:
  `crates/taskboard-ui-slint/src/lib.rs:2991`
  `keymap_adapter_tests::insert_intent_opens_the_create_row_and_escape_returns`
  — `i` opens the create row (insert mode), a board/stack model push
  via `handle.refresh()`, then Escape must return to normal mode. With
  the wrap, the refresh's model push rebuilds the field's subtree and
  the final `press("\u{1b}")` dead-ends ("mode.normal returned" — the
  exact failure signature of all three withdrawal cycles).
- **The wrap-less pin** (must stay green, and must stay green WITH the
  wrap — it is the clip-focus regression):
  `crates/taskboard-ui-slint/src/lib.rs:3473`
  `view_component_tests::editing_dispatches_nothing_and_exits_via_escape`
  — the "mid-edit refresh keeps the flow open" case (#1419 body's
  repro test).
- **Wrap-dependent pins, withdrawn with the wrap, re-land with it**:
  `tests/probes/columns_seek_probe_tests.rs` (from `91c33b23`) —
  overflow landing moves the columns element's window x negative,
  clamped to the content; the fitting regime stays at rest.
- **Guard rails against focus stealing** (must stay green):
  `detail_focus_journeys` rows 3/4 (the restore pins that killed the
  refocus-nonce) and `a_dead_zone_click_settles_the_create_flow` (the
  click-out settle — the guard against any steal-back).

---

## 2. Mechanism analysis

### 2.1 Previously tried and rejected (do NOT re-propose without new evidence)

| Mechanism | Why it failed |
|---|---|
| EditingFacts-count-guarded `refocus_view` | The counts are stale at echo time — the remounted instance's init publish claims focus before the deferred release lands. |
| `changed width` mount-settle re-grab in `text-field.slint` | Same deferral race: the width `changed` handler runs in the same deferred change-handler pass as the release; ordering inside the pass is not controllable from slint. |
| `EditingFacts.refocus-nonce` bumped post-push and from the absorb branch, consumed with and without a `has-focus` guard | The re-grab never sticks (still inside the pre-echo window), and bumping on *every* pass steals focus onto visible-but-unfocused autofocus fields in flowless surfaces (`detail_focus_journeys` row3/row4 red — the nonce was removed from the landed state). |
| Bare `clip: true` Rectangle instead of Flickable | Bisected failure (#1419): the clip property ALONE drops focus on refresh — the visibility tracker (`window.rs:1548`) sees the focused item behind a clip and drops it. |
| Slint-side translation without any clipped/scrolled container | The translation law (#1419): a layout under a non-layout parent has its geometry bindings overridden by the default-geometry pass; no pan carrier exists. |

### 2.2 The unifying reading (new, from the vendored source)

Every rejected slint-side mechanism scheduled its re-grab as a slint
`changed` handler or a property binding — all of which run in the SAME
deferred change-handler pass as the focus release
(§1.3). Within that pass the release wins (or the ordering is
undefined), so the re-grab never sticks. What has never been tried is
scheduling the re-grab **in a later event-loop turn than the
change-handler pass** — or not letting the rebuild happen at all.

### 2.3 Candidate mechanisms NOT yet tried

**A. Adapter-owned, timer-ordered re-grab (the law's remount half,
adapter-side).** The adapter owns the push pass and the
`refresh_push_in_progress` window. Before the pass it records "a
focused create/rename field is mounted and the flow is open"; after
arming, it schedules `slint::Timer::single_shot(TimerMode::SingleShot,
Duration::ZERO, …)` (timer precedent in this crate:
`crates/taskboard-ui-slint/src/lib.rs:472-498`, the tooltip driver).
A zero-duration timer callback executes in a later event-loop turn
than `run_change_handlers()` (timers are processed after the queued
property-handler pass) — so the deferred release AND the visibility
drop have both landed, and the window's focus item is empty; the
timer's callback then invokes a new slint global seam
(`TryRefocus.refocus-create()` — a global function that calls the
*currently mounted* create/rename field's `grab-focus()`
(`text-field.slint:135`), resolving "currently mounted" per-instance,
so the rebuilt instance is the target). Consumed-once semantics (the
arm clears on fire, on a user-intent seam, and on settle) — no
per-pass bumping, so the nonce's steal-the-focus failure mode is
structurally absent. Risk: timer-vs-change-handler ordering is
documented behaviour but must be *proven* on the testing backend
(experiment E1); the timer must run on the slint thread (the adapter
already does).

**B. Keep the field instance alive — hoist the create/rename field out
of the rebuilt subtree.** The defect's precondition is "a model push
that rebuilds the field's subtree". The per-column create field
(`board-layout.slint:655`) is instantiated inside the
`for column in root.stacks` model; every push rebuilds it. The
stack-create field already has a **board-level, flowless-surface
precedent** (`board-layout.slint:764`, and the empty-board variant
`:278`, compact leg `:880`): a single `if (create-open): TextField`
outside the columns model. Hoisting the inline create field (and the
per-column rename field) to one board-level instance whose x/width
bind to the active column's resolved geometry means the model push
never unmounts the focused field — no deferred release, no echo, no
re-grab; the Flickable wrap then re-lands with no focus machinery
change at all. Risk: bigger diff (geometry anchoring over a Flickable
viewport whose `content-x` shifts; goldens; the `create-committed`
callback must carry the column index from the state, not the loop
index — it already does: `create-committed(int, string)` at
`board-layout.slint:119`). Also changes no user-visible behaviour —
the field renders identically at the same position.

**C. Solve the clip problem without the wrap.** Exhausted: bare
`clip: true` is bisected-fatal (#1419), non-Flickable translation is
blocked by the translation law, and every widget whose model diff
doesn't rebuild the subtree still carries a clip (fatal per the
bisect). Rejected — no new evidence.

**D. Suppress the teardown-loss dismissal signal with a typed guard.**
This IS the landed absorb seam (§1.4) — necessary but insufficient:
it stops the false settle, not the focusless window. No further work.

**E. Synchronous echo delivery (upstream slint).** #1490's fix-prompt
shape 1. Correct but not actionable in-scope (needs an i-slint issue
and an upstream release); record as a `backlog:` follow-up regardless
of A/B outcome.

### 2.4 Ranking

1. **B (hoist the field)** — strongest evidence: it removes the
   defect's precondition entirely; precedent exists in the same file;
   no timing reasoning at all. Higher diff risk (geometry + goldens).
2. **A (timer-ordered re-grab)** — smallest diff; mechanically well
   founded (§2.2); one unproven ordering assumption that E1 settles in
   ≤30 min.
3. D (landed), E (upstream), C (exhausted).

**Discriminating experiments** (each ≤ ~30 min, on a scratch branch
off this chunk's branch, every cargo command through
`scripts/memory-gate.sh`):

- **E1** — re-land `91c33b23`'s wrap verbatim, add the adapter timer
  re-grab (candidate A) with the consumed-once arm, then run:
  `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint
  insert_intent_opens_the_create_row_and_escape_returns
  editing_dispatches_nothing_and_exits_via_escape` plus the seek
  probe. Green ⇒ candidate A is viable.
- **E2** — same wrap verbatim, replace the per-column create field
  with the hoisted board-level instance (candidate B, geometry-only
  first pass: no rename-field hoist yet), then run the same two tests.
  Green ⇒ candidate B is viable.
- E1/E2 are independent; run E1 first (cheaper). If both green, choose
  A (smaller diff) unless stage 2 finds a geometry blocker; if only E2
  is green, choose B; if both red, execute the rollback (§6).

---

## 3. #1502's role: INDEPENDENT

#1502's harness gap (cannot focus a named inner `FocusScope` and
inject raw key events headless) is **not a prerequisite and not a
rider** for #1490:

- All verification of #1490 happens through the keymap-adapter path
  (`driver.rs:237` `press_stroke` → window-level key dispatch), which
  already works headless and is exactly the path the defect breaks
  (focusless window dead-ends the stroke). The red test
  (`lib.rs:2991`) needs no new harness seam.
- #1502's gap concerns widget-internal `key-pressed` handlers (the
  colour picker's nudges) — a different layer, a different fixture.
  Per AGENTS §5 ("one behavior, one test, lowest layer"), bolting it
  onto this chunk would mix layers for no verification gain.

**Decision**: #1502 stays out of this chunk entirely; it is already a
standalone `backlog:` issue and gets dispatched in a future testing
round. This plan only guarantees it is not *blocked* by #1490.

---

## 4. The decision — recommended approach

**Candidate B preferred, candidate A accepted** — decided by E1/E2
(§2.4). Both candidates satisfy every acceptance criterion below;
the experiments pick which lands. Stage 2 re-lands the wrap
(`91c33b23`, verbatim) FIRST on this branch, then lands the chosen
focus mechanism on top, so every intermediate commit is
reviewable: the wrap commit alone is expected to leave
`insert_intent_opens_the_create_row_and_escape_returns` red (recorded
reality, #1490) — the follow-up commit turns it green. **The branch
must be green at handoff; a red intermediate commit is acceptable
only if the next commit on the branch fixes it before handoff.**

### Acceptance criteria (all mandatory)

1. **The #1419 ruling's observable behaviour**: the columns row is a
   Flickable; in the overflow regime clipped columns stay reachable by
   pointer and keyboard and the per-column seek consumption
   (`FocusScrollHint`) moves a landing column into view (clamped); in
   the fitting regime the row stays at rest.
2. **The red test green**:
   `keymap_adapter_tests::insert_intent_opens_the_create_row_and_escape_returns`
   passes WITH the wrap present.
3. **The absorb seam intact**:
   `seams/task.rs:44-67` unchanged in behaviour; the echo window arms
   at every push pass and `apply_outcomes` as landed.
4. **No dismissal echo**: a mid-edit refresh keeps the flow open
   (`view_component_tests::editing_dispatches_nothing_and_exits_via_escape`,
   `lib.rs:3473`, with the wrap) and fires exactly one
   `text_flow_focus_lost` per genuine dismissal — the echo is
   absorbed, never settles.
5. **No weakened tests**: `detail_focus_journeys` rows 3/4,
   `a_dead_zone_click_settles_the_create_flow`, and the whole existing
   suite pass unmodified; the seek probe pins re-land from `91c33b23`
   unchanged (only the position read re-derivation already in that
   commit is allowed).

### Invariants

- The UI layer holds no business logic or state mutation (AGENTS §2):
  the re-grab decision lives in the adapter (`taskboard-ui-slint`
  src), the slint side exposes only a mount-resolution seam.
- The consumed-once law for any refocus arm (the R-d one-shot law):
  an arm fires at most once and only while a flow is open; it never
  fires in flowless surfaces (the nonce's failure mode).
- The width resolver's oracle input is unchanged (D-1365a): it reads
  the viewport width app-side.
- Board goldens unaffected (they held through `fead35c2`).

### Verification commands (stage 2 gate; dispatcher owns the full §10 suite)

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint \
  insert_intent_opens_the_create_row_and_escape_returns
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint \
  editing_dispatches_nothing_and_exits_via_escape
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint \
  columns_seek_probe
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint detail_focus_journeys
scripts/run-quality-gates.sh cargo nextest run --workspace
```

### Rollback shape (the honest fallback)

If E1 **and** E2 fail: leave the wrap withdrawn at `origin/main` state;
push the branch with the experiments' evidence appended to a report
under `.artifacts/reports/2026-11-27-focus-remount-wrap-experiments-report.md`;
comment the full evidence on #1490 (including the §1.3 change-handler
deferral source reading and the upstream-slint question for shape E)
and open a `backlog:` issue for the upstream i-slint synchronous-echo
question. The wrap is not forced. No PR opens for a wrap that cannot
hold the pins.

---

## 5. Stage prompts

### Discipline (binding — `docs/agent_dispatch.org`; restated verbatim in both prompts)

> A subagent is killed after ~600s without observable activity.
> **Long-command discipline**: (1) *The coverage trio never runs inside
> a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks
> (automatic) plus `scripts/run-quality-gates.sh cargo nextest run
> --workspace` (flock-safe; background + poll with useful work between
> polls if needed); the dispatcher runs the full AGENTS §10 suite
> between stage 2's landing and stage 3's ship. *Flock-deferral rule:
> when the gate lock is contended and no other useful work remains,
> never block — commit all landed work, report "gate deferred: lock
> held by <holder>", and end the stage; the dispatcher runs the
> gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR,
> report — the dispatcher owns `gh pr checks --watch`,
> `mergeStateStatus CLEAN`, teardown, and at-merge issue closures.
> (3) *Any command expected over ~5 minutes runs backgrounded* with
> short interleaved polls and useful work between polls — commit all
> landed work BEFORE any wait. Large scratch under /var/tmp, never
> /tmp. (4) **Every agent-launched cargo invocation routes through
> `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts
> restate this discipline verbatim.*

### 5.1 Stage 2 (IMPLEMENTATION) prompt — paste verbatim

```
You are the STAGE 2 (IMPLEMENTATION) agent for the chunk "UI focus
remount vs the Flickable wrap" in the taskboard repo (GitHub:
D4id4los/taskboard), executing the accepted plan
`.artifacts/plans/2026-11-27-focus-remount-wrap-plan.md` (read it in
full first). Issues: #1490 (p1), #1502 (p3 — OUT OF SCOPE for this
chunk, independent). Origin ruling: #1419 option A (issue comment
7172599685). This wrap has been attempted and withdrawn three times —
your plan names every rejected mechanism; re-proposing one without the
plan's new-evidence reading is a plan violation.

Discipline (binding — docs/agent_dispatch.org): A subagent is killed
after ~600s without observable activity. **Long-command discipline**:
(1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3
agent's gate is the pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe; background + poll with useful work between polls if
needed); the dispatcher runs the full AGENTS §10 suite between stage
2's landing and stage 3's ship. *Flock-deferral rule: when the gate
lock is contended and no other useful work remains, never block —
commit all landed work, report "gate deferred: lock held by <holder>",
and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope
ends at PR-open*: push, open the PR, report — the dispatcher owns
`gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and
at-merge issue closures. (3) *Any command expected over ~5 minutes
runs backgrounded* with short interleaved polls and useful work
between polls — commit all landed work BEFORE any wait. Large scratch
under /var/tmp, never /tmp. (4) **Every agent-launched cargo
invocation routes through `scripts/memory-gate.sh`** — nothing bare.
(5) *Stage prompts restate this discipline verbatim.*

Environment: work ONLY inside
`/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-focus-remount-wrap`
(branch `fix/ui-focus-remount-wrap`, already created off `origin/main`
by stage 1). The main checkout must not be modified. Do not touch any
other `taskboard-WT-*` directory.

Your job, in order:

1. **Re-land the wrap verbatim**: apply commit `91c33b23`'s four files
   (`crates/taskboard-ui-slint/ui/compositions/board-layout.slint`,
   `crates/taskboard-ui-slint/tests/columns_seek_probe.slint`,
   `crates/taskboard-ui-slint/tests/probe_composition.rs`,
   `crates/taskboard-ui-slint/tests/probes/columns_seek_probe_tests.rs`)
   onto the branch — `git cherry-pick 91c33b23` (or revert
   `8ca5291c`) is the expected shape; resolve nothing unless real
   conflicts appear. Commit as
   `fix(ui): re-land the board columns Flickable wrap per the clip-focus law (#1419 option A)`
   (this intermediate commit is EXPECTED to leave
   `keymap_adapter_tests::insert_intent_opens_the_create_row_and_escape_returns`
   red — recorded reality, #1490; do not weaken the test, AGENTS §5).
2. **Run experiment E1 then E2** exactly as plan §2.3/§2.4 prescribes
   (scratch branches off the chunk branch if you need to park a
   losing attempt; every cargo command through the memory gate; each
   experiment ≤ ~30 min). Record the outcomes in the final report.
3. **Land the winning mechanism** on the chunk branch per plan §4:
   candidate A (adapter-owned, timer-ordered, consumed-once re-grab
   via a new slint global seam) or candidate B (hoist the per-column
   create field out of the rebuilt subtree to a board-level instance
   anchored to the active column). Whichever wins:
   - The re-grab decision NEVER lives in slint bindings or `changed`
     handlers (plan §2.2: same deferred pass as the release — the
     documented failure of all three prior attempts).
   - Consumed-once arm only (the R-d one-shot law); never a per-pass
     bump (the nonce's steal failure, `detail_focus_journeys` row3/4).
   - The absorb seam `seams/task.rs:44-67` keeps its landed behaviour
     (plan acceptance criterion 3).
   - TDD: write/adjust the failing-case evidence FIRST (the two plan
     §4 tests plus the seek probe), watch them red on the wrap-only
     branch state, then drive them green. When a newly written test
     fails, treat a bug in the production code as equally likely as a
     bug in the test (AGENTS §5); never weaken a correct test; no
     text assertions; assert typed outcomes.
4. **Gate**: pre-commit hooks run automatically. Then
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (background + poll; flock-deferral rule applies). The coverage
   trio, clippy full suite, and the rest of AGENTS §10 belong to the
   dispatcher between stage 2 and stage 3 — do NOT run them.
5. Commit everything (house commit hygiene, AGENTS §8: every commit
   stands alone — a correct build, `<type>(<module>): <subject>`,
   no WIP noise — rebase-merge law). Report: E1/E2 outcomes, the
   winning mechanism and why, files touched, the gate status (or
   "gate deferred: lock held by <holder>"), any `backlog:` issues
   opened (search first, standing authorization AGENTS §7).

Diff-shape rules: the wrap commit must match `91c33b23`'s four-file
shape (nothing else); the mechanism commit touches only the files its
candidate needs (A: adapter src + one slint global + tests; B:
board-layout.slint + the create-committed callback plumbing + tests).
No drive-by refactors, no reformatting, no new dependencies, no
changes outside `taskboard-ui-slint` (and the plan/report artefacts).
The two tests that must not have been weakened:
`lib.rs:2991 insert_intent_opens_the_create_row_and_escape_returns`
and `lib.rs:3473 editing_dispatches_nothing_and_exits_via_escape` —
their assertions stay byte-identical. If BOTH experiments fail, stop,
do not force anything: leave the branch at the wrap-only state,
write the evidence report the plan §6 rollback names, comment the
evidence on #1490, open the upstream-slint `backlog:` issue, and
report the rollback. SPDX headers on every new .rs file (AGENTS §6);
`#![forbid(unsafe_code)]` holds.
```

### 5.2 Stage 3 (REVIEW) prompt — paste verbatim

```
You are the STAGE 3 (REVIEW) agent for the chunk "UI focus remount vs
the Flickable wrap" in the taskboard repo (GitHub:
D4id4los/taskboard), reviewing the branch
`fix/ui-focus-remount-wrap` against the accepted plan
`.artifacts/plans/2026-11-27-focus-remount-wrap-plan.md` (read it in
full). Issues: #1490 (p1), #1502 (p3 — out of scope here). Origin
ruling: #1419 option A (issue comment 7172599685). This wrap was
attempted and withdrawn three times; review specifically guards
against a fourth silent recurrence.

Discipline (binding — docs/agent_dispatch.org): A subagent is killed
after ~600s without observable activity. **Long-command discipline**:
(1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3
agent's gate is the pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe; background + poll with useful work between polls if
needed); the dispatcher runs the full AGENTS §10 suite between stage
2's landing and stage 3's ship. *Flock-deferral rule: when the gate
lock is contended and no other useful work remains, never block —
commit all landed work, report "gate deferred: lock held by <holder>",
and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope
ends at PR-open*: push, open the PR, report — the dispatcher owns
`gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and
at-merge issue closures. (3) *Any command expected over ~5 minutes
runs backgrounded* with short interleaved polls and useful work
between polls — commit all landed work BEFORE any wait. Large scratch
under /var/tmp, never /tmp. (4) **Every agent-launched cargo
invocation routes through `scripts/memory-gate.sh`** — nothing bare.
(5) *Stage prompts restate this discipline verbatim.*

Environment: work ONLY inside
`/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-ui-focus-remount-wrap`.
The main checkout must not be modified. Do not touch any other
`taskboard-WT-*` directory.

Review checklist (verify in the worktree, not by trust):

1. **Wrap fidelity**: the wrap commit matches `91c33b23`'s four-file
   shape (`board-layout.slint`, `columns_seek_probe.slint`,
   `probe_composition.rs`, `columns_seek_probe_tests.rs`), geometry
   verbatim; the seek probe pins are the unweakened originals.
2. **Mechanism legality**: the focus mechanism commit follows plan §4
   — no re-grab decision in slint bindings or `changed` handlers; a
   consumed-once arm only; no per-pass nonce bump; the absorb seam
   (`src/adapter/seams/task.rs:44-67`) behaviourally unchanged; the
   re-grab decision lives in the adapter (AGENTS §2 SRP line: the UI
   never embeds business logic or state mutation).
3. **Acceptance criteria**: all five of plan §4 hold — run the plan's
   verification commands (memory-gated, background + poll; the
   workspace gate through `scripts/run-quality-gates.sh`, flock
   rules). The two tests that must not have been weakened —
   `lib.rs:2991 insert_intent_opens_the_create_row_and_escape_returns`,
   `lib.rs:3473 editing_dispatches_nothing_and_exits_via_escape` —
   have byte-identical assertions; `git diff origin/main -- '*lib.rs'`
   plus reading them confirms.
4. **Test quality** per AGENTS §5: no text assertions, no quality
   ceilings, deterministic by construction, lowest layer, no pins of
   "today's behavior".
5. Diff-shape: no drive-by refactors, no reformatting, no new
   dependencies, nothing outside `taskboard-ui-slint` except plan/
   report artefacts; every new .rs file carries the correct SPDX
   header (MIT OR Apache-2.0) and `#![forbid(unsafe_code)]` holds.

**Code-quality mandate** (stage 3): beyond functional review, evaluate
(a) code quality: naming, comment discipline (comments state
constraints, not narration), idiomatic use of the approved stack, no
dead code or speculative generality, test quality per AGENTS §5;
(b) the Single Responsibility Principle: the crate-role boundaries of
AGENTS §2 are the SRP line — the UI never embeds business logic or
state mutation; (c) a single level of abstraction per function body.
Findings are fixed on the branch.

Then: push the branch (`git push -u origin fix/ui-focus-remount-wrap`
— the dispatcher's chunk mandate authorizes this push), open the PR
titled per the plan's subject with the plan, the #1490 mechanism
summary, and the experiment outcomes in the body, and END YOUR STAGE
at PR-open. Report the PR URL, the review findings and their fixes,
the gate status (or "gate deferred: lock held by <holder>"), and any
`backlog:` issues opened (search first — standing authorization,
AGENTS §7). Do NOT watch checks, do NOT merge, do NOT remove the
worktree — the dispatcher owns all of that.
```

---

## 6. UX journey plan decision

**No new UX journey plan is needed.** Per `docs/ux_planning.org` the
mandate covers work that *adds or changes a user-visible capability*.
This chunk restores already-ruled, already-planned behaviour: the
Flickable wrap + per-column seek is #1419 option A / #1365, planned in
`.artifacts/plans/2026-11-25-severing-geometry-completions-plan.md` §3
and `.artifacts/plans/2026-11-25-clip-sentinel-law-plan.md` §4 (both
accepted; the ruling is recorded on #1419), and the focus mechanism
fixes a regression (keys dead-ending) rather than introducing a verb,
badge, view, or setting. The chunk cites those accepted plans; the
seek probe pins re-land as the behaviour's verification. Stated so the
stage-3 `scripts/ux-coverage.sh` question has a recorded answer.

---

## 7. Rulings requested

1. **Experiment-gated dispatch vs straight dispatch.** The plan
   dispatches stage 2 conditional on E1/E2 (run inside stage 2, not as
   a separate pre-stage). Recommended default: proceed as written —
   the experiments are ≤30 min each and stage 2 is instructed to stop
   and execute the rollback rather than force a losing mechanism.
   *Proceeding on the default.*
2. **A-vs-B preference if both experiments are green.** Default: land
   A (smaller diff) unless a geometry blocker appears. If the operator
   prefers the structural fix (B — no focus machinery change, but a
   bigger slint diff), say so before dispatching stage 2.
   *Proceeding on the default.*
3. **#1502 as independent** (not a rider): default is out-of-scope for
   this chunk, dispatched in a future testing round.
   *Proceeding on the default.*
4. **Push authorization for stage 3**: the stage-3 prompt instructs
   the branch push + PR-open (chunk handoff shape per AGENTS §8);
   this plan records that as the chunk's authorized push. Any further
   push (review fixes) follows the same rule.

---

## 8. Estimated size

- Wrap re-land: 206 lines (the `91c33b23` shape, four files) — M.
- Candidate A: ~80–150 lines (adapter arm + timer + one slint global +
  seam wiring + tests) — S/M.
- Candidate B: ~150–300 lines (board-level field, geometry anchoring
  over the Flickable viewport, callback plumbing, goldens check) — M.
- Either way the chunk is **M** (one branch, one PR, wrap commit +
  mechanism commit + any review fixes).
