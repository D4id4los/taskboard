# [Plan] Widget anatomy remainder — round 5 chunk E4

- **Date**: 2026-11-23
- **Kind**: stage-1 implementation plan — five widget-anatomy backlog
  items (#1121, #1125, #1126, #1127, #1128), each issue body
  normative. The frame is the O-6 law read in both directions (no
  speculative knobs; no speculative growth whose trigger has not
  fired), with revisit-or-record as a legitimate per-item outcome.
- **Origin** (normative): scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §E; dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1 E4 row + §9 E4 entry; the five issue bodies; the S8 anatomy
  family (`.artifacts/plans/2026-11-21-widget-anatomy-bundle-plan.md`,
  the round-4 chunk that already visited these items; the
  widget-anatomy origins
  `.artifacts/plans/2026-11-20-small-widget-primitives-plan.md` and
  `.artifacts/plans/2026-11-20-menu-shell-plan.md`).
- **Preconditions verified 2026-11-23**: `scripts/worktree-doctor.sh`
  exit 0 (98G free / ~10G available); worktree created with
  `git worktree add -b refactor/widget-anatomy-remainder
  ../taskboard-WT-refactor-widget-anatomy-remainder` off the fetched
  `origin/main` (`d7646762`).
- **Status**: READY for stage 2 (as a verify-and-record chunk).

## 0. The headline outcome (read this first)

**Round-4 chunk S8 already visited every item in this chunk**, one day
before E4 was dispatched: it completed #1128 (commit `a84bcbf6`,
"the MenuShell cap-rows knob retires (O-6)", landed on `origin/main`
2026-11-22, issue closed COMPLETED 2026-11-22T06:39Z) and posted
dated evidence-refresh comments on the four stays (#1121/#1125/
#1126/#1127). This chunk's diagnosis re-reads the corpus fresh from
the new tip (`d7646762`) and confirms every S8 ruling still holds —
nothing in the widget family moved between `a53cab50` and `d7646762`
(the intervening commits are journey/test work outside `ui/widgets`
and `ui/compositions`).

**E4 therefore lands zero production code.** Its deliverables are:
this plan (the recorded decisions), a dated re-verification pass over
the four stay issues' trigger conditions, and the dispatcher-facing
closure recommendation. Forcing code here would manufacture churn the
O-6 law forbids in both directions.

## 1. Per-item decisions (each: diagnosis on the tip, then the ruling)

### #1121 — rule-list hint caption vs SectionHeader anatomy

- **Diagnosis on `d7646762`**: `ui/compositions/rule-list.slint`
  lines 78–81 remain the conditional wrapped hint caption
  (`if (root.hint != ""): RoleText`, colour
  `root.muted ? Semantic.warning : Theme.text-secondary`) plus the
  separate conditional muted-indicator row; the add affordances still
  live in the presets row below. The hint row has **no inline
  affordances** — the revive trigger (stated in the issue body and in
  the 2026-11-22 S8 refresh comment) has not fired.
- **Decision: RECORD (stay).** Converting to
  `ui/widgets/section-header.slint` would still change paint (title
  colour, wrap) and structure — the zero-delta bar of the origin
  chunk (small-widget-primitives plan §1.3/§3 row 16) still forbids
  it, and the census deviation entry (`docs/ux-surface-census.org`
  W5) stands. No code. Issue stays OPEN with its trigger.

### #1125 — AnchoredOverlayHost pointer-anchor + fill-to-floor

- **Diagnosis on `d7646762`**: `ui/compositions/action-menu.slint`
  still carries the fill-to-floor height law
  (`height: min(self.preferred-height, self.available-height)`,
  line ~108) and `ui/app-window.slint` (lines ~142–144, ~1542–1610)
  still hand-mounts the ActionMenu window with the pointer-anchor
  centring law (`KeyboardState.menu-pointer-anchored/x/y`, round-1
  D13 clamp formula at ~1555–1563). `ui/widgets/anchored-overlay.slint`
  has **no** pointer-anchor or fill-to-floor mode — every other
  overlay consumer still fits the anchor-rect + content-height
  contract. The revive trigger (a third consumer needing either
  mode) has not fired; there is still exactly one consumer.
- **Decision: RECORD (defer-with-evidence).** Growing the host with a
  second anchor mode for one consumer is the speculative
  customization point O-6 forbids — the same law that retired
  `cap-rows` applies in the growth direction (per the S8 refresh
  comment). No code. Issue stays OPEN with its trigger.

### #1126 — DropdownTone → the Tone ladder (Tone.accent arm)

- **Diagnosis on `d7646762`**: `ui/widgets/dropdown.slint` lines
  45–137 keep `DropdownTone` distinct with its local `tone-color`
  (the `accent → Semantic.focus-ring` arm at line 136);
  `ui/theme.slint` `Tone` + `tone-color` (lines ~165–204) still has
  **no accent arm**. The accent arm's consumer set is still exactly
  the one family recorded in the S8 refresh: `DropdownRow.tone`,
  consumed transitively by `menu-row.slint::leading-tone` (line 46)
  via `DropdownFacts.tone-color` — the round-9 W6 flyout chunk made
  `MenuRow` a *reader of the existing vocabulary*, not a second
  independent consumer of an accent colour. The revive trigger (a
  second consumer adopting the accent arm) has not fired; if it ever
  does, the conversion now has a slightly wider blast radius than the
  issue body's original recipe (both `DropdownRow.tone` **and**
  `MenuRow.leading-tone` convert, then `DropdownTone` deletes).
- **Decision: RECORD (defer-with-evidence).** Growing the shared
  nine-arm Tone vocabulary for one consumer remains speculative. No
  code. Issue stays OPEN with its trigger; the blast-radius note
  above is the chunk's new information for the revisit recipe.

### #1127 — labels roster rows onto a form-row primitive

- **Diagnosis on `d7646762`**: `ui/compositions/labels-overlay.slint`
  `LabelRow` is unchanged — two pointer-grade IconButtons, the armed
  strip, the swatch square: form-row anatomy, not menu-row. The
  destination primitive class still does not exist; per the S8
  refresh the question is chained to #1096 (the declined
  `LabeledFieldRow` form-row variant, re-deferred 2026-11-22 with
  still exactly two label+editor consumer families). #1096's trigger
  has not fired.
- **Decision: RECORD (stay, chained to #1096).** Absorbing onto
  `MenuRow` would strip the rows' verbs (a behavior change with no
  ruling) or grow `MenuRow` across widget classes (the O-6 taxonomy
  keeps them distinct). No code. Issue stays OPEN, chained to
  #1096's trigger.

### #1128 — MenuShell's cap-rows knob

- **Diagnosis on `d7646762`**: **already completed and closed.**
  Commit `a84bcbf6` removed the knob: the unconditional cap
  `min(row-count·space-6, space-6·6)` is the shell's law
  (`ui/widgets/menu-shell.slint` line 29), the header comment
  records the 2026-11-22 retirement and the defer law for any future
  uncapped consumer, `cap-rows` has zero remaining references
  workspace-wide (grepped: `.slint`, `.rs` — including the G-2
  passthrough probe leg, retired with the knob), and the issue is
  CLOSED COMPLETED as of 2026-11-22T06:39Z.
- **Decision: VERIFY-AND-RECORD (no-op for this chunk).** The
  dispatch plan §9 E4 row was written against the pre-S8 world; the
  work landed under round-4's branch. Stage 2 performs no action
  beyond the corpus gate; the dispatcher should simply confirm the
  issue's closed state at merge (it is already closed — no at-merge
  closure owed).

## 2. Stage-2 instructions

1. Re-run the diagnosis greps of §1 on the then-current tip and
   confirm each ruling still holds (expected: trivially yes — the
   widget family is quiescent).
2. Post one dated evidence-refresh comment per stay issue
   (#1121/#1125/#1126/#1127) recording the 2026-11-23 re-verification
   against `d7646762` and citing this plan (search-first per §7;
   append, do not duplicate). For #1126 include the
   `MenuRow.leading-tone` blast-radius note from §1.
3. No code changes; no UX journey plan needed (dispatch plan E4 row:
   no UX plan; nothing user-visible changes — nothing changes at all).
4. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` per the [D] discipline (commit landed
   work before any wait; if the lock is held, report "gate deferred:
   lock held by <holder>" and end the stage — with a docs-only diff
   the run is confirmatory, not load-bearing).

## 3. Stage-3 instructions

Review the plan-vs-landed correspondence only: the refresh comments
posted, the citations, the absence of code churn (any production
diff in this chunk is a plan violation). Re-run the UX coverage gate
only if — contrary to plan — any user-visible surface changed.

## 4. At-merge bookkeeping (dispatcher-owned)

- #1128: already closed COMPLETED; nothing owed.
- #1121/#1125/#1126/#1127: **stay OPEN** — they are armed revisit
  records, not completed work; closing them would delete the
  trigger conditions the O-6 defer law lives on. Recommend no
  closure at merge.

## 5. Risks

- **Chunk-shape drift**: stage 2 "implementing" nothing can look like
  under-delivery against the dispatch plan's M sizing. The recorded
  reality is that S8 (round 4) preempted E4; the deliverable is the
  verified record. If the operator wants code out of E4, that is a
  re-scope decision, not a stage-2 initiative.
- **Tip drift**: if a later round-5 chunk lands widget-family changes
  before E4 runs, a stay's trigger could fire; §2 step 1 is the
  tripwire.
- **Gate cost**: the full nextest run is pure confirmation on a
  docs-only diff; the flock-deferral rule applies with priority.

## 6. Estimated diff size

Docs-only: this plan (~150 lines) + four issue comments. Production
diff: **0 lines**. Test diff: **0 lines**.
