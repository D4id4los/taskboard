# [Plan] UX Remediation — Chunk 6: P3 posture & polish (the phase closure)

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 6 of the UX remediation phase
  (framework plan §6 phase 5, the last remediation chunk and the
  phase's closure). Planning only — no application code in this stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§7/§8
  (the shared execution model, the long-command discipline, and §7's
  priors are binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 MINOR
  table (T-24..T-47) and §2 (the doctrine-bar context); home issues
  #807–#830 plus the walkthrough MINORs #838–#846; the nine owning UX
  plans (`2026-11-14-{board-stack-management,task-editing,task-lifecycle,habit-family,recurrence-family,timers-alarms,capture-search,review-walk-reflection,settings-connections}-ux-plan.md`);
  the census `docs/ux-surface-census.org`; `docs/ux_planning.org` §2.4–§3.
- **Status**: READY — chunks 1–5 merged (verified: `main` @ `26b2a013`
  carries r1–r5); the triage report accepted 2026-11-15; worktree
  `../taskboard-WT-docs-ux-r6-p3-posture` on branch
  `docs/ux-r6-p3-posture`.
- **Binding inputs of stage 2**: every decision D-1..D-26 below and the
  resolution ledger (§5), the test catalogue (§6), the commit
  sequencing (§7). Deviations need a recorded reason in the stage-2
  report.
- **Non-goals** (dispatch §7, restated): re-opening any accepted
  classification; new feature work beyond the named fixes; the
  CLI-twin implementation (T-24); the mandate flip (phase 6, its own
  chunk after this PR merges).

## 1. Re-derived remaining state (what chunks 1–5 already answered)

Chunks 1–5 resolved T-1..T-23 (their home issues #784–#806 closed with
the merged PRs). No MINOR finding T-24..T-47 was consumed by an
earlier chunk: the dispatch explicitly preserved T-38's busy lane
("the busy lane's posture untouched — T-38 is chunk 6's") and the
r5 plan cites the doctrine bar as a *constraint*, not a resolution.
The chunks' walkthrough protocols filed new MINORs #838–#846: these
are **not** chunk-6 findings (they post-date the accepted triage) —
they stay on their issues; this chunk touches only #842 (the r1
erratum, D-24) and #768 (the coverage-gate debt ruling, D-25).

## 2. The doctrine bar — T-46 (#829) — D-1

**RULING → docs** (the dispatch's prior; upheld — the five positives
are walkthrough evidence with anchors, and homing them as a bar is
exactly their MINOR classification's intent).

Landing shape:

1. `docs/ux_planning.org` gains a new subsection after §2.5:
   `** 2.6 The doctrine bar (recognition-over-recall positives)` —
   the five positives **verbatim** from their owning plans' walkthrough
   notes (the evidence text is preserved exactly; the bar adds only
   the framing sentence "recorded 2026-11-15 by the phase-5
   walkthroughs (triage T-46, #829); every new surface and every
   review round holds to these; a regression against any of them is a
   review finding, not a style difference"):
   - **BF-W-N2** (board-stack plan §10 W-N2): "the context menus order
     destructive rows last and grey disabled verbs (`menu.rs:210-227`,
     `menu.rs:68-71`)".
   - **TE-W-N3** (task-editing plan §10 W-N3): "the staged clear
     echoes 'clears the due date' before commit (`due_edit.rs:170`)".
   - **RW-W-N1** (review-walk plan §10 W-N1): "the stage-then-close
     Escape preset (`presets.rs:312`) is command-free by design".
   - **SC-W-N4** (settings-connections plan §10 W-N4): "non-anchor
     accounts show disabled actions with honest captions, not lanes
     (the pilot's R-3 ruling, #723)".
   - **SC-W-N2** (settings-connections plan §10 W-N2): "the pickers'
     seam names are consistent (`*-picked`,
     `settings-view.slint:672-696`)".
2. The census header (below the Status line) gains one posture-note
   sentence pointing at `docs/ux_planning.org` §2.6 (T-46, #829) as
   the regression bar every census diff is judged against.
3. #829 closes as completed on merge (resolution = the ruling landed).

No census **cell** changes (T-46 records posture, not capability).

## 3. Resolution ledger — every finding T-24..T-47 exactly once

Fix = small code change (typed-outcome tested, §6) + owning-plan
amendment + census cell update + issue closes on merge.
Ruling = recorded posture in the census (cell note or slice block
note, dated, citing the authority) + issue closes on merge.
Deferral = reason recorded on the issue (comment) + issue stays open,
**no milestone**.

| Finding | Issue | Fate | Decision id |
|---|---|---|---|
| T-24 CLI read-only twin family | #807 | **DEFERRAL** | D-2 |
| T-25 GUI habit edit/remove (recorded authority) | #808 | RULING (no-op) | D-3 |
| T-26 inline card editing | #809 | DEFERRAL | D-4 |
| T-27 mute/snooze scoping | #810 | RULING | D-5 |
| T-28 card tracking badge display-only | #811 | DEFERRAL | D-6 |
| T-29 undo-window invisibility | #812 | DEFERRAL | D-7 |
| T-30 capture lands global Inbox | #813 | DEFERRAL | D-8 |
| T-31 flow replaced unannounced | #814 | **FIX F-4** | D-9 |
| T-32 alarm-window anchor-aware shape undocumented | #815 | RULING | D-10 |
| T-33 cancel family no undo | #816 | RULING (no-op) | D-11 |
| T-34 reflection NotUndoable | #817 | RULING (no-op) | D-12 |
| T-35 review ledgers non-interactive | #818 | RULING (no-op) | D-13 |
| T-36 on-restart knob feedback | #819 | DEFERRAL | D-14 |
| T-37 board-create affordance split | #820 | DEFERRAL | D-15 |
| T-38 busy-lane refusal without reason | #821 | **FIX F-1** | D-16 |
| T-39 settings exit posture | #822 | RULING | D-17 |
| T-40 count badge dual-purpose unlabeled | #823 | DEFERRAL | D-18 |
| T-41 habit log acknowledgement (+ make-up note) | #824 | **FIX F-2** (the HB-W-N1 half; HB-W-N2 ruling) | D-19 |
| T-42 recurrence display notes | #825 | RULING (no-op) | D-20 |
| T-43 timer create acknowledgement (+ attention + mute posture) | #826 | **FIX F-3** (the TA-W-N1 half; W-N2/W-N3 rulings) | D-21 |
| T-44 review semantics in code comments | #827 | RULING | D-22 |
| T-45 login alias + re-anchor rationale | #828 | RULING (no-op) | D-23 |
| T-46 doctrine bar | #829 | RULING → docs (§2) | D-1 |
| T-47 blank reflection record silent | #830 | **FIX F-5** | D-26 |

Headline: **5 fixes, 11 rulings (6 of them no-op records), 8
deferrals.** 17 issues close as completed on merge (#808, #810,
#814–#818, #821, #822, #824–#830); 8 stay open with the deferral
reason commented (#807, #809, #811–#813, #819, #820, #823 — and #814
closes, so the open set is #807, #809, #811, #812, #813, #819, #820,
#823).

## 4. Decisions

### D-2 — T-24 (#807) DEFERRAL: the CLI read-only twin family
The dispatch's prior, upheld. The decision rule's MINOR arm stands: no
GUI journey depends on any CLI twin (the census Sweep-CLI evidence).
Implementing ~30 read-only CLI cells is polish of the command surface,
not a remediation of a journey defect — and §7 names it "deliberately
not dispatched here". Stage 2 comments the deferral reason on #807
(citing this plan + the dispatch §8); the issue stays open, no
milestone; no census change (the CLI column already records the
absences).

### D-3 — T-25 (#808) RULING (no-op): habit edit/remove authority stands
Seed rulings R-2/R-3 are standing authority, restated in the ledger.
The census habit cells already carry the note; stage 2 comments the
confirmation on #808 (citation only) and the issue closes as
completed — the finding asked for the record, and the record is the
resolution.

### D-4 — T-26 (#809) DEFERRAL: inline card editing is feature work
Card click → detail is the recorded posture (`adapter.rs:8349,8391`);
chunks 3's remediation put full editing controls in the detail view.
Inline editing on cards is a new editing surface (new seams, new
journeys), not a small fix — out of scope per the non-goals. Deferred;
if ever scheduled it owns a UX journey plan first (post-phase-6 law).

### D-5 — T-27 (#810) RULING: mute/snooze scoping is typed design
Banner-and-rule-local scoping (snooze banner-only; `ReminderUnmute`
keyless at `presets.rs:110-113`) is a deliberate register posture
recorded under TE-2. Adding the missing key would be keyboard-parity
feature work, not posture repair. Census: the mute/snooze register
cells gain a dated ruling note ("banner-and-rule-local by design;
triage T-27 ruling 2026-11-15") without changing their `U` markers.
Issue closes on merge.

### D-6 — T-28 (#811) DEFERRAL: card start/stop affordance is feature work
The display-only Tracking badge matches T-46's display-atom posture
(the r4 plan records card badges as display atoms); start/stop from
cards duplicates the detail-view and chip seams — a new affordance,
not a fix. Deferred.

### D-7 — T-29 (#812) DEFERRAL: the undo-window boundary element
Making the 10-entry window visible before a refusal names it requires
a new persistent UI element (badge/hint) — feature-shaped. The typed
`NothingToUndo` refusal (chunk-1-era posture) remains the boundary's
signal. Deferred; a future status-bar affordance is the natural home.

### D-8 — T-30 (#813) DEFERRAL: board-targeted capture is feature work
Board-scoped capture (a target picker in the funnel) changes the
capture journey's shape — the capture-search plan records the
global-Inbox posture as its US-1/S3 evidence. Deferred.

### D-9 — T-31 (#814) FIX F-4: announce the flow replacement
**Shape**: when capture or the palette opens over a live flow (the
`router.rs:49` replacement; `adapter.rs:2979`), the adapter sets the
existing status-notice lane with the typed fact "a live flow was
replaced" before the overlay mounts — the same status-notice pattern
chunk 5 landed for the invoked disabled menu row. No confirmation
stage, no new element: the settle rule still bounds data-at-risk; the
fix adds only the announcement. Owning plan amendment: capture-search
plan §10 W-N2 gains a dated supersession note (the walkthrough
observation is remediated; the journey's escape column is unchanged).
Census: Capture slice `capture.quick` / `palette.search` cells gain
the dated note. Test: §6 T-4.

### D-10 — T-32 (#815) RULING: the anchor-aware shapes are deliberate
D-P11 (chunk-2 plan D-8) is the recorded design: alarm anchors carry
Dismiss/Snooze/Cancel, reminder anchors add Done/Mute. The "missing"
rows are entity-honesty, not a defect; surfacing the difference as UI
text would explain a design the shapes already express. Census:
alarm-window block note dated 2026-11-15 citing T-32 + D-P11. Issue
closes on merge.

### D-11 — T-33 (#816) RULING (no-op): the cancel family's no-undo is deliberate intent
`CancelAdhocAlarm` ends the entity by design; no undo class covers it;
the finding itself records the posture. Census already carries it.
Issue closes as completed — the record is the resolution.

### D-12 — T-34 (#817) RULING (no-op): the reflection family's NotUndoable rationale stands
`undo.rs:238-245` + DR-7 D-J7 record the rationale (the inverse of a
record is a tombstone; the family's own verbs are the recovery
surface). Unlike T-15 (remediated in chunk 5), this posture was
classified MINOR *because* the rationale is recorded. No change.
Issue closes as completed.

### D-13 — T-35 (#818) RULING (no-op): ledgers are presentation-scoped displays
Display-without-control on displays is presentation-scoping (the
finding's own words); activation on ledger rows would be feature work.
No change. Issue closes as completed.

### D-14 — T-36 (#819) DEFERRAL: per-knob deferred-application feedback
The facts badge exists (the finding's own doubt record: "recognition
requires finding it") and the classified defect was accepted MINOR
under never-inflate. A per-knob hint is a new element per 18-variant
knob family — feature-shaped. Deferred with the doubt citation.

### D-15 — T-37 (#820) DEFERRAL: the board-create affordance split
Aligning the settings name-less pick to the Board surface's typed
field changes the settings journey (a text field in the pick row, a
preview/undo stage) — feature work. The two shapes serve different
grammars (compose-in-place vs configure-quick). Deferred; the
settings-connections plan keeps SC-5 as the recorded observation.

### D-16 — T-38 (#821) FIX F-1: the busy refusal carries its reason
**Shape**: the settings busy case (`adapter.rs:5708-5710`) renders
`SettingsOutcome::Rejected` with `None` detail. Change the busy arm to
render the typed busy posture through the same lane chunk 5 landed for
env-locked refusals (`settings_status::status_text` — commit
`9b5c1bb4`'s pattern): the outcome stays a typed variant (no new
enum), the rendered detail becomes `Some(<busy posture>)` so the user
is told a request is pending. No text asserted in tests — the test
asserts the structural fact (§6 T-1). Owning plan amendment:
settings-connections plan SC-7 gains the dated remediation note.
Census: Settings slice busy-lane cell note. Test: §6 T-1.

### D-17 — T-39 (#822) RULING: the exit posture stands
Back-seam-only exit with in-flight requests resolved by the reply
lane's drop guard (idempotent snapshots, no data loss) is the recorded
posture; a preset binding for "exit settings" would be feature work.
Census: Settings slice `app.settings` row gains the dated note. Issue
closes on merge.

### D-18 — T-40 (#823) DEFERRAL: the count badge's dual purpose
Labeling the move-flow aim tone requires a caption element keyed to a
transient flow state — new surface element, feature-shaped; the tone
flip itself is correct aim feedback. Deferred.

### D-19 — T-41 (#824) FIX F-2 (+ partial ruling): the +1 acknowledgement
**Shape** (the HB-W-N1 half): `LogHabit`'s FlowOutcome routes to the
status-notice lane with the typed surplus fact (the logged count
delta) — same pattern as F-3/F-4/F-5; no new transient widget. The
HB-W-N2 half (make-up targeting carried by the action description) is
**RULING (no-op)**: the action description *is* the recognition
surface the plan designed; the finding records the posture, no change.
Owning plan amendment: habit-family plan §10 W-N1 gains the dated
remediation note (W-N2 a no-op note). Census: `habit.log` Today cell
note. Test: §6 T-2.

### D-20 — T-42 (#825) RULING (no-op): the recurrence display notes stand
RC-W-N1 was recorded *for the triage* — "the census cell stands" is
the finding's own resolution; RC-W-N3's description-carried semantics
are the designed recognition surface. No change. Issue closes as
completed.

### D-21 — T-43 (#826) FIX F-3 (+ partial rulings): the created-timer acknowledgement
**Shape** (the TA-W-N1 half): `CreateAdhocAlarm`'s FlowOutcome routes
to the status-notice lane (typed fact: the created occurrence's
target) — the acknowledgement lands with the next render's row as its
recognition anchor, matching F-2's pattern. The TA-W-N3 half
(request-never-focus attention, `lib.rs:778-786`) is **RULING
(no-op)** — environment-dependent attention is the recorded design;
and TA-W-N2 (the notification mute as the banner's `NotUndoable(
ReminderSurface)` twin) is **RULING (no-op)** — the class is already
the task-editing plan's recorded posture. Owning plan amendment:
timers-alarms plan W-N1 remediated; W-N2/W-N3 no-op notes. Census:
`alarm.create_adhoc` Today cell note. Test: §6 T-3.

### D-22 — T-44 (#827) RULING: the review semantics get census homes
The carry's due-hygiene and the planner's atomicity contract stay in
code (correct — semantics belong to the engine), but the census gets
them as dated cell notes so a future reviewer reads the semantics
without opening `adapter.rs:2214-2249`: the `review.carry_tomorrow`
and `plan.commit_place` Review cells note "due dates never touched"
and "one atomic edit, one undo step" citing T-44. Issues close on
merge.

### D-23 — T-45 (#828) RULING (no-op): the alias and the rationale stand
The deprecated `login` alias is recorded at `cli.rs:56` and in the
census; the single-request re-anchor rationale living in the payload
docs is the right home for engine rationale. No change. Issue closes
as completed.

### D-24 — #842: the r1 plan's false `SetBoardArchived` undo claim
**Erratum via dated PC entries** (the supersession pattern the
board-stack plan's PC-4 established — accepted analysis is never
rewritten in place):
- `.artifacts/plans/2026-11-15-ux-r1-completability-chunk-plan.md`
  gains a `## 12. Plan corrections` section: **PC-1 (2026-11-15,
  chunk-6 stage 1)** recording that D-1/D-4's claim "`SetBoardArchived`
  is journal-eligible (`undo.rs:324`)" is **false** — the command
  falls in the BoardCeremony arm (`UndoFacts::NotUndoable(
  NotUndoableReason::BoardCeremony)` at `main` @ `d7a8173c`), so the
  unarchive's escape is the re-archive path, not `AppUndo`; the
  shipped behavior (the r1 PR's unarchive seam) is unaffected — the
  defect was in the plan's justification text only.
- The board-stack-management plan's `## 11. Plan corrections` gains
  **PC-5** with the same correction for its US-11 carriage, pointing
  at #842 and the r1 PC-1.
- No census change (the census records behavior, which was correct);
  #842 closes as completed on merge (the erratum is the resolution).

### D-25 — #768 / PC-2: the residual coverage-gate MISSING-rows debt
**Recorded deferral ruling, homed on the existing issue.** The gate's
journey-step token check still cannot map descriptive test names on
pre-framework seams; retro-fitting mapping tables for already-landed
plans is gate tooling work, not phase-5 remediation, and §7 forbids
new feature work. Stage 2 comments the ruling on #768 (citing this
plan; the MISSING-row residual is bounded to pre-phase-5 plans — all
post-r1 chunk plans gate green with step tokens, per the landed r1–r5
runs); the issue stays open, no milestone.

### D-26 — T-47 (#830) FIX F-5: the blank record's typed no-op feedback
**Shape**: the all-blank reflection record's typed guard
(`adapter.rs:2300-2339` region — the gentle end at `adapter.rs:2455`)
currently leaves only the walk stage as signal. Route the guard's
no-op path to the status-notice lane with the typed "nothing recorded"
fact — F-2/F-3's pattern. Owning plan amendment: review-walk plan
RW-W-N3 gains the dated remediation note. Census: `reflection.record`
Review cell note. Test: §6 T-5.

### D-27 — Phase closure edits
1. **Census status line**: replace the current Status with
   "*phase 5 complete (remediation); the framework §6 phase-5 exit is
   recorded; **phase 6 (mandate activation) is next** — a tiny docs
   chunk flips AGENTS.md §7's advisory wording to mandatory*
   (2026-11-15)", preserving the previous status text as a "Previously:"
   paragraph (the census status convention).
2. **`docs/ux_planning.org` §3 activation table**: the phase-5 row's
   State becomes "complete 2026-11-15 — the remediation chunks
   merged; findings resolved (fix/ruling) or explicitly deferred with
   homes"; phase 6 stays "pending" and the current-state line notes
   phase 6 is next.
3. **Completion bookkeeping** (dispatch §8): stage 3 closes the 17
   resolved findings' issues as completed with the PR URL; the 8
   deferred issues keep the deferral comments and no milestones; the
   walkthrough MINORs #838/#839/#840/#843/#845 and the flaky-test
   watchlist #846 are untouched by this chunk (their own issues
   govern).

## 5. Fix design constraints (binding for stage 2)

- One pattern, four echoes: F-2, F-3, F-4, F-5 all use the existing
  status-notice lane (the chunk-5 landed pattern, commit
  `509be8ec`); no new widgets, no new commands, no new model fields
  beyond what the lane already carries. If a target surface lacks a
  visible notice region, STOP that fix and record the deviation —
  degrade to deferral with the reason, do not invent a new element.
- UI-crate purity: all fixes live in `taskboard-ui-slint`
  presentation wiring; zero IO, zero business logic.
- No text assertions: every test asserts typed outcomes
  (`matches!(..)` / structural facts — detail `is_some`, notice flag
  set, payload variant), never message strings.

## 6. Test catalogue (one behavior, one test, lowest layer)

| # | Behavior | Test (unit, in the owning module) | Typed assertion |
|---|---|---|---|
| T-1 | F-1 (D-16): the busy lane's refusal carries the pending posture | `view_state.rs` settings-outcome test: drive the busy case, read the rendered outcome | outcome is `Rejected` **and** detail is `Some(..)` (structural; the old shape was `None`) |
| T-2 | F-2 (D-19): `LogHabit` outcome sets the status notice | `view_state.rs` (or the habit wiring module's existing outcome test site): dispatch the log outcome | the notice lane's state flips to set with the surplus payload variant |
| T-3 | F-3 (D-21): `CreateAdhocAlarm` outcome sets the status notice | same site, the adhoc-create outcome | notice set with the created-occurrence payload variant |
| T-4 | F-4 (D-9): opening capture/palette over a live flow sets the notice | adapter/router-level: mount a live flow, fire the overlay open | notice set **and** the overlay route still opens (both facts, one test) |
| T-5 | F-5 (D-26): an all-blank reflection record sets the no-op notice | the walk's record path, all-blank fixture | no `RecordReflection` dispatched **and** notice set with the no-op variant |

Engine-side behavior is already snapshot-tested in `taskboard-state`;
none of these tests duplicate it. No journey-level duplication
(higher layers test routing only).

## 7. Commit sequencing (each standalone, `<type>(<module>):` subjects)

1. `fix(ui): the settings busy refusal carries its typed reason`
   — F-1 + test T-1 + the SC-7 amendment + census note.
2. `fix(ui): habit log and adhoc-timer outcomes echo the status notice`
   — F-2 + F-3 + tests T-2/T-3 + the HB/TA plan amendments + census
   notes.
3. `fix(ui): overlay opens over a live flow announce the replacement`
   — F-4 + test T-4 + the capture plan amendment + census note.
4. `fix(ui): a blank reflection record echoes its typed no-op`
   — F-5 + test T-5 + the review-walk amendment + census note.
5. `docs(ux): the doctrine bar lands as the regression bar`
   — D-1 (ux_planning.org §2.6 verbatim + census posture note), the
   rulings' census notes (D-3/D-5/D-10/D-11/D-12/D-13/D-17/D-20/D-22/D-23),
   the census status line + ux_planning.org §3 closure (D-27).
6. `docs(plan): the r1 undo-claim erratum and the gate-debt ruling`
   — D-24 (both PC entries) + D-25's record (the #768 comment itself
   is stage-2's `gh` action, permitted outside git).

Stage 2 then runs `scripts/ux-coverage.sh <each amended plan> --gate`
and the fast gate (`scripts/run-quality-gates.sh cargo nextest run
--workspace`; **never the coverage trio inside the agent** — dispatch
§1 long-command discipline). Stage 3 does the issue bookkeeping
(D-27.3), the full review, ship.

## 8. Risks

- **The status-notice lane's per-surface visibility** is assumed from
  the chunk-5 pattern; if a target surface (Today rows, the walk, the
  overlay-open path) lacks the lane's render, the fix degrades to a
  recorded deferral (§5) — worst case the chunk shrinks to F-1 + docs,
  still a complete ledger.
- **`Rejected`-detail typing** (F-1): the detail's type must not
  become a stringly channel — stage 2 reuses the existing typed
  detail carrier; a new enum variant is acceptable, a `String` is not.
- **Verbatim-bar drift** (D-1): the five positives must be copied
  from the owning plans at implementation time, not paraphrased;
  stage 3 spot-checks the anchors.
- **Issue-bookkeeping timing**: closures happen at stage 3 ship time,
  not before the PR — a closed-but-unmerged issue would lie about
  state.

## 9. Estimated diff size

Five small fixes (~15–40 lines each + one test each, ~250–400 lines
total), two plan erratum sections, the doctrine-bar subsection, census
notes across ~12 cells/blocks + the status line: **M** (~500–700
lines across ~10 files).

## 10. Gate

Pre-commit hooks on every commit; the fast gate
(`scripts/run-quality-gates.sh cargo nextest run --workspace`) green
before handoff to stage 3; `scripts/ux-coverage.sh --gate` green on
every amended plan. The coverage trio is stage-3/CI territory only.
