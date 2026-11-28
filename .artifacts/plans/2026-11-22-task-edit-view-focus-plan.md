# [Plan] Round 5 chunk A2 — the task edit view focus cluster (#1205, #1203, #1204, #1207)

- **Date**: 2026-11-22
- **Kind**: stage-1 chunk plan (diagnosis-first per round law [L](c)) —
  branch `fix/task-edit-view-focus`, worktree
  `../taskboard-WT-fix-task-edit-view-focus` (off `origin/main`
  `0946b220`; doctor exit 0, 169G free, 13.2G MemAvailable).
- **Normative sources**: scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §A; dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1/§3 (chunk A2 row + the dispatcher verdict checkpoint); issues
  #1205 (p1, systemic), #1203, #1204, #1207 (p3s). Standing laws
  read: the N-3 focus-restoration contract (r8 chunk 2,
  `.artifacts/plans/2026-11-17-ui-r8-focus-restoration-plan.md` —
  D-F1 origin fact, the §2 census, the D-F1 ordering law), the K2
  directional law (`.artifacts/plans/2026-11-19-directional-focus-plan.md`
  §1 law paragraph + §4 per-view table), the landed undo-line
  machinery (`plan_journaled_command`/`plan_undo`,
  `UndoFacts::NotUndoable(reason)` classification, the UF1/B1-era
  plans' typed-refusal surface).

## 1. Ground truth — the diagnosis (stage-1 findings, all read at
   `0946b220`)

Line numbers below are from the worktree at the base commit; the
issue's cites predate several refactors and are superseded by these.

### 1.1 #1205 — TWO mechanisms, one view

**Mechanism A (Enter theft): a context-granularity Return row with no
focus attribution.** The emacs preset carries
`(ActionId::TaskTitleEdit, ContextId::TaskDetail, &["Return"])`
(`src/keymap/presets.rs:936`). `ContextId::TaskDetail` is the whole
view's context: whenever a Return press is not consumed by a nearer
scope, the resolver resolves it to `TaskTitleEdit` and the view opens
title-edit mode — regardless of which widget holds focus. The nearer
scope that consumes Return for *some* widgets is the insert-mode text
editor (its `accepted` handler / the editor keyscope's in-field
commands), which is exactly why the theft hits "most non-title
widgets": every widget that is *not* a text field (the tracking
buttons, priority cells, add-label/add-repeat rows, time-entries
opener, timer-entry buttons) lets Return fall through to the context
row. The vi preset binds Return at `ViNormal` (a different context)
and Standard does not bind Return at `TaskDetail` at all — so the
mechanism is present in all presets only in the sense that the
emacs row is the one that fires; under vi/Standard the same press is
simply inert (no activation), which is the same attribution gap seen
from the other side. The row's own round-3 comment ("Return at detail
is unambiguous once task-detail entry no longer auto-arms") records
the assumption this bug falsifies: the detail is now a full form, and
Return at detail is no longer unambiguous.

**Mechanism B (focus restore): the detail's anchor resolution
collapses every opener onto the title cell.**
`Adapter::resolve_anchor` (`crates/taskboard-ui-slint/src/adapter.rs`
~2141) maps *any* `OriginAnchor::Task` while `ViewId::TaskDetail` to
the tuple `(4, -1, -1)` — anchor kind 4, the title cell (census rows
14/15's "the title/description cell"). The only escape is the
`DescriptionEditor` seam special case in `push_focus_restore` (kind 4
→ 5). So every overlay opened from the detail view — due/start date
picker, priority picker, track entries, recur editor — records a
correct `FocusOrigin` (the D-F1 machinery works; minting, one-shot,
last-open-wins are all fine) and then *restores it wrong*: kind 4
fires `invoke_focus_detail_cell(4)`, and in the view only the title
display claims kinds 4/5 (`ui/views/task-view.slint:785-792` —
`title-display.request-value-focus()`). The opener's identity is not
"replaced" anywhere in the bookkeeping; it is **discarded at the
resolution step** because the anchor vocabulary (`OriginAnchor`) is
coarser than the detail view's widget geography. The title is not a
grab scope and holds no default-focus assignment beyond the view
root's `init => self.focus()` (`task-view.slint:459`, which re-anchors
the *key root*, not the title); the title lands on every restore
because kind 4 names it unconditionally.

**One-sentence root cause (the [L](c) deliverable):** the task edit
view funnels both Enter and overlay-exit restore through view-level
defaults that predate the view becoming a full form — a
context-wide `Return → TaskTitleEdit` preset row with no per-widget
attribution, and an anchor resolution that maps the detail's single
`Task` anchor kind onto the title cell for every opener seam. Fix the
mechanisms (attribution + per-seam cell resolution), not per-widget
patches — exactly the issue's ask.

### 1.2 #1203 — the K2 machinery is registered; the gap is downstream

Contrary to the survey's first candidate, **the edit view's
directional rows are NOT missing from the K2 registration**:

- Preset rows exist in all three families: Standard arrows
  (`presets.rs:510-513`), emacs C-f/b/n/p
  (`presets.rs:1335-1340`), vi j/k/h/l at `ViNormal`
  (`presets.rs:1429-1432`).
- The per-view interpretation table has TaskDetail rows
  (`adapter.rs::interpret_directional`, ~6113: verticals walk the
  rule cursor via `walk_rule_cursor`, everything else is the control
  walk), and `directional_control_step` (~6029) hands TaskDetail to
  the traversal tail unless the rule cursor owns the stroke.

So the chords resolve to effects; what is unverified is **which layer
kills them between effect and visible focus movement**. Three named
suspects, in the order stage 2 must test them:

1. **In-field interception while any text field holds focus** — the
   editor keyscope's in-field cursor commands lawfully consume
   C-f/C-b/C-n/C-p when an editor is focused (the K2 US-1 AC-1 text
   guard, `keyscope.rs:431-434`); this is *correct* while a field is
   focused, but the user report includes strokes with non-field
   widgets focused.
2. **The context chain while an edit flow is open** — with
   `Flow::EditTitle`/`EditDescription`/date editors mounted, the
   chain's top context may shadow the `TaskDetail` rows.
3. **The traversal tail's stop inventory inside the form** — the
   synthetic-Tab walk (or its mask-armed replacement, r8 D-W2) may
   have no/partial stops among the form's widgets, so a *resolved*
   `FocusDirectional` moves focus somewhere invisible.

**Decision D-A2-3 (diagnosis-first pin):** stage 2 writes the layer
test FIRST — dispatch each of the four emacs chords in a fixture with
(a) a non-field widget focused, (b) a text field focused, (c) an edit
flow open — asserting the typed outcome at each layer (effect
resolved / intercept consumed / traversal injected). The failing layer
names the fix. If the fix turns out to be "the walk has no stops in
the form", the fix is a stop inventory in `task-view.slint` + the
walk facts, not a preset change; a preset change is only lawful as a
dated retune comment. This composes with K2 (its table is the
authority this chunk makes actually reachable); no contradiction.

### 1.3 #1204 — the armed flag exists; the editor's styling does not read it

The model half landed with r8 D-F5: `TaskDetailModel.title-armed` /
`description-armed` (`task-view.slint:82-88`) and the closed display
row latches the walk ring (`description-display` `ring-latched:
TaskViewWalkFacts.stop-id == 11`, stop-id 11 at :1291-1295). The
`MultiLineEditor` widget renders its focus ring from its private
`input-has-focus` mirror only (`ui/widgets/multi-line-editor.slint:86,
114-115`) — the armed push (`description-armed`) never reaches the
editor's border, so an armed-but-not-focused editor (the walk/restore
case, and the moments around the mount handoff) shows no highlighted
state. **Decision D-A2-4:** add an `armed: bool` input to
`MultiLineEditor`, bind the border on
`input-has-focus || armed`, feed it from `root.task.description-armed`
at the description editor mount; the title's `TippedField` gets the
same audit in the same commit (one commit — the "census rider fixes
any further armed-field gaps under the same seam" letter of r8
D-F5). Presentation test asserts the flag reaches the widget (typed).

### 1.4 #1207 — the never-silent pin; deep diagnosis stays deferred

The issue itself authorizes deferring the deep root-cause hunt to the
undo-v2 GUI (the branch-picker makes the watermarks visible); the
dispatch plan's acceptance for this chunk is the **typed-outcome
pin**, not the hole's fix: every edit-write path's undo is *applied
or a typed refusal, never silent*. The suspect inventory stage 2
works from (all read at base):

- The engine surface: `crates/taskboard-state/src/core.rs`
  (`journal()` → `UndoView::derive`, ~line 152) and
  `crates/taskboard-domain/src/undo.rs`
  (`plan_journaled_command` / the `UndoFacts::NotUndoable(reason)`
  classification table ~lines 206–361: `BoardCeremony`,
  `ReminderSurface`, `TrackingSurface`, `HabitSurface`).
- The UI half: `ActionId::AppUndo` (`actions.rs:255`, registry
  ~1067, describe ~759) and its router arm.
- The issue's named candidates: view-local staged edits that never
  journal a command, and edit-path commands classified
  `NotUndoable` (cf. the deliberate imports arm the issue cites at
  `src/actor.rs:1335-1336` — pre-consolidation numbering; its
  successor lives in the state/UI crates at base).

**Decision D-A2-7 (two-step, bar = [L](a)):**

1. **The pin (in this chunk):** a state-tier test drives an edit
   sequence that includes the reported shape (set a start date, plus
   further edits), dispatches the undo verb, and asserts the journal
   cursor moved (applied) **or** a typed
   `UndoFacts::NotUndoable(reason)` surfaced — the assertion is
   `matches!` over the typed outcome, never text, never
   "something happened". A red pin here is a bug per AGENTS §5; if
   the pin exposes the silent path, fix it in the same chunk **only
   if** the fix is view/action-tier (a staged edit that never
   journals, a UI arm swallowing the typed refusal). 
2. **The escalation boundary:** if the pin's root cause lives inside
   the journal/watermark machinery itself (`plan_journaled_command`
   / `plan_undo` semantics, the classification table's edit-path
   classes), that is an undo-contract change → **GATE-ESCALATED**
   to the dispatcher per the dispatch plan's verdict checkpoint; the
   pin lands, the fix does not.

## 2. Law-compliance verdicts (the [L](c) record)

| Law | Verdict | Record |
|---|---|---|
| N-3 (r8 focus restoration, D-F1) | **Enforced, not amended.** | The restore fix (§3 D-A2-1b) makes the census's per-site opener restore *actually true* in the detail view; today's kind-4 collapse is a violation of the census rows 1/5/6/10's letters ("the invoking field"), not a competing reading. The D-F1 ordering law (origin resolution before the view-scope refocus; the view-scope call only when the origin resolved to `View`) is untouched. |
| K2 (r9 directional law) | **Composed, not patched around.** | The §4 table stays the authority; #1203's work is making its TaskDetail rows reachable (dispatch/traversal layer), zero preset/interpret-table semantic changes. The #1205 Enter fix does not touch directional ids or their contexts. |
| Undo-line contracts (UF1/B1 era; the `NotUndoable(reason)` typed surface) | **Composed; escalation boundary recorded.** | The pin asserts the existing typed contract; no machinery change is planned. A machinery-level root cause is GATE-ESCALATED (§1.4). |
| The round-3 W-SM-d assumption ("Return at detail is unambiguous") | **Dated supersession, recorded here.** | The preset row's justification comment is falsified by the view's current shape; the fix amends the assumption with a dated retune comment beside the changed row(s) (the house census-supersession pattern), not silently. |

**No GATE-ESCALATED items at stage 1** — all planned fixes compose
with the standing laws. The one conditional escalation is §1.4's
machinery branch.

## 3. The fixes (stage-2 spec)

### D-A2-1a — Enter attribution (#1205 mechanism A)

- The context row `(TaskTitleEdit, TaskDetail, ["Return"])` loses its
  blanket role: Return is attributed to the focused element first.
  The house shape for "which element is focused" is the pushed-facts
  seam: stage 2 extends the detail's focus facts (the
  `FocusFacts`/`TaskViewWalkFacts` family) with the focused stop's
  **activate action id**, pushed by the view when slint focus rests
  on a form widget that owns an activation (the value rows'
  `edit-asked`, the buttons' `clicked`, the display cells' verbs).
  The resolver's intercept consults it: a claimed Return dispatches
  the claiming widget's own action (click ≡ key, the us6_s2 equality
  pattern); no claim → the context row fires as today (title-edit
  stays reachable from the view at large — Enter with no form widget
  focused still arms the title, which is the row's legitimate
  remaining role).
- The emacs row keeps its binding (its semantics become "Return with
  no focused claimant"); the supersession comment is added beside it.
- The title's own Enter (inside the open title editor) is the
  editor's `accepted` — untouched.

### D-A2-1b — per-seam detail restore cells (#1205 mechanism B)

- Generalize the existing `DescriptionEditor` special case into a
  **seam → detail-cell table** in `push_focus_restore`: each
  `FocusSeam` that opens from the detail resolves to the opener's own
  cell kind (the due/scheduled editor → the due/scheduled value
  cells; the priority pickers → the importance/urgency cells; track
  entries → the tracking verbs; recur → the add-repeat cell; title →
  kind 4 as today; description → kind 5 as today). Anchor kinds mint
  alongside the view's stop ids (the view already names stops:
  2 tracking, 3 title, 5/6 effort area, 11 description — stage 2
  numbers the remaining cells on the same ladder).
- `task-view.slint` gains the matching `anchor-kind` claimants per
  cell (the `init`/`changed nonce` request-value-focus pattern the
  title row already carries, :785-792).
- **The widget-inventory table** (§4) is the test surface; every row
  is both an Enter-attribution row and a restore row.

### D-A2-2 — the #1203 layer test + fix per the named layer (§1.2).

### D-A2-3 — the #1204 armed-ring input (§1.3).

### D-A2-4 — the #1207 never-silent pin (§1.4).

## 4. The widget-inventory test table (#1205; the stage-3 [L] trap (a) oracle — a missing row is a finding)

Every widget the issue inventoried, one row each; the test is
table-driven (one fixture, per-row expected tuples — typed facts,
never text). "Enter" = the emacs preset's Return stroke with the
widget focused; "restore" = open the row's overlay, dismiss it, ask
where slint focus rests (`FocusFacts` tuple + the
`WindowInner::focus_item` read where meaningful, the r8 §1 oracle).

| # | Widget (view) | Overlay it opens | Enter pre-fix (expected red) | Enter post-fix | Restore post-fix lands on | FocusFacts kind |
|---|---|---|---|---|---|---|
| 1 | Task Title (display cell, stop 3) | title editor (in-view flow) | arms title-edit (legitimate — the row's remaining role) | unchanged | the title cell | 4 |
| 2 | Task Title (editor open) | — (is the overlay) | commits (accepted) | unchanged | the title cell | 4 |
| 3 | Due date value cell | date picker (`FocusSeam::DueEditor`) | stolen → title-edit (red) | opens the due editor | the due value cell | new (due) |
| 4 | Start date value cell | date picker (`FocusSeam::DueEditor` scheduled leg) | stolen (red) | opens the scheduled editor | the scheduled value cell | new (scheduled) |
| 5 | Start tracking button | none | stolen (red) | starts tracking (click ≡ key: same command, the us6_s2 equality) | — (no overlay) | — |
| 6 | Stop tracking button | none | stolen (red) | stops tracking | — | — |
| 7 | Importance cell | priority picker (`PriorityDue`) | stolen (red) | opens the picker | the importance cell | new (priority-importance) |
| 8 | Urgency cell | priority picker (`PriorityScheduled`) | stolen (red) | opens the picker | the urgency cell | new (priority-scheduled) |
| 9 | Add label row | labels overlay (`LabelsOverlay`) | stolen (red) | opens labels | the add-label cell | new (labels-opener) |
| 10 | Add repeat row | recur editor (`RecurEditor`) | stolen (red) | opens recur | the recur cell | new (recur) |
| 11 | Open time entries | track entries overlay (`TrackEntries`) | stolen (red) | opens entries | the entries-opener cell | new (track-entries) |
| 12 | Timer entry buttons (track form) | timer setter (`TimerSetter`) | stolen (red) | the button's own action | the timer-entry cell | new (timer) |
| 13 | Description (editor open) | — | commits (accepted) | unchanged | the description cell | 5 (existing) |
| 14 | Description (display, stop 11) | description editor | stolen (red) | opens the description editor | the description cell | 5 |
| 15 | Effort field (stop 6, existing kind-6 restore) | effort editor | stolen (red) | the field's own commit/verb | the effort cell | 6 (existing — must stand) |

Fallback leg (per family, one run): vanished anchor (delete the task
under an open overlay) → the view scope, never no-control — the
`us1_s2` letter re-pinned for the detail's new kinds.

Row 12's Test nomination (issue #1259, the 2026-11-28 harness
journeys plan D-#1259):
`detail_focus_journeys.rs::row12_entries_overlay_walk_lands_on_the_add_affordance_and_return_opens_the_form`
— the entries overlay's proven stop inventory (rows → "Add time
entry") and Return on the add affordance opening the seeded entry
form (`track_form_open`, typed). The row's literal reading ("Set a
timer" opener → `TimerSetter` inside the overlay) was geographically
wrong: the overlay carries no such control — the opener lives on the
today view's Timers section — so the overlay-leg journey pins the
overlay's own walk instead (correction recorded on #1259).

## 5. TDD order & commit sequencing (each commit standalone)

1. `test(ui): the detail widget-inventory table — the stolen-Enter and
   collapse-restore pins` — §4's table, red on rows 3–12/14/15's
   Enter legs and every restore leg except 13; the #1207 never-silent
   pin (state tier) lands here too if it is red-on-arrival, else with
   3.
2. `fix(ui): per-seam detail restore cells` — D-A2-1b + the FocusFacts
   kind minting + the view claimants; green restore legs.
3. `fix(ui): Return is attributed to the focused detail widget` —
   D-A2-1a + the claimant facts + the intercept arm + the dated
   supersession comment at `presets.rs:936`; green Enter legs; the
   title-legitimate rows still green.
4. `test(ui): the directional chords' layer oracle at the detail` —
   D-A2-2's layer test; then the fix commit it names (its own commit,
   message per the named layer).
5. `fix(ui): the armed ring reaches the detail's editors` — D-A2-3.
6. `test(state): undo of an edit is applied or typed-refusal, never
   silent` — D-A2-4's pin (fold into 1 if already written there); a
   view-tier hole gets its `fix(state|ui):` commit here; a
   machinery-tier red is recorded GATE-ESCALATED and the pin stands
   red **only** under AGENTS §5's leave-failing rule with the report
   note — otherwise the pin is written to assert the contract at the
   boundary that already holds it and the escalation carries the
   red.

Gate steady state: pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace` (the
coverage trio is the dispatcher's).

## 6. Risks & watch items

- **The intercept's Return arm is shared surface** (the resolver's
  precedence chain): the claimant fact must be consulted *after* the
  text-editor scopes (insert mode keeps winning inside fields — the
  K2 US-1 AC-1 negative tests must stand) and *before* the context
  row. The r8 clamp's negative tests and the RW-W-N1 Escape-preset
  positives are the regression nets.
- **New anchor kinds touch `FocusFacts`** (a pushed global): the
  `seam_inventory.rs`/coverage sides update in the same commits; the
  r8 `us1` family must stand byte-identically for its existing
  kinds.
- **The stop ladder renumbering** (§3 D-A2-1b) risks the walk probe's
  order pin (r8 §6): appended kinds only; a re-baseline is a
  recorded event, never a silent retune.
- **#1203's fix is unknown until the layer test names it** — the
  diff estimate below assumes the traversal-stop variant (the
  likeliest per §1.2); a preset-tier outcome would add a retune
  comment + snapshot regen instead.
- **#1207's pin may be red for machinery reasons** — the escalation
  boundary is pre-agreed (§1.4); the stage does not widen into the
  journal.

## 7. Diff estimate

~200 src (adapter resolver + restore table) + ~120 slint (view
claimants, editor armed input) + ~30 presets/comments + ~600 tests
(table fixture, layer oracle, pin) ≈ **0.9–1.1k LOC** across ~6–7
commits.

## 8. Open questions — all resolved

| Question | Decision |
|---|---|
| Is the K2 TaskDetail registration missing? (the issue's survey candidate) | **No** — rows/table/control-step all present (§1.2); the deadness is downstream; the layer test names the layer before any fix. |
| Is the title a grab scope / default-focus assignment? | **No** — the view root self-focuses the key root (`task-view.slint:459`); the title wins restores via the kind-4 collapse and Enter via the context row. No grab-scope change is needed. |
| Does the Enter fix remove the `TaskTitleEdit` Return row? | **No** — the row stays as the no-claimant fallback (title-edit remains keyboard-reachable); its semantics are narrowed by the claimant seam, with the dated supersession comment. |
| Does the restore fix amend the census table (r8 §2)? | **Yes, additively** — the detail's per-seam cell column joins the census as an addendum row-note in this plan (§4); r8's letters are unchanged. |
| Fix #1207's hole now? | **Pin now; fix only if view/action-tier; machinery-tier = GATE-ESCALATED** (§1.4), per the issue's own deferral note and the dispatch plan's checkpoint. |
| UX journey plan needed? | **No** — bug fixes restoring already-accepted journeys (N-3's census, K2's table); the dispatch plan's A2 row records "no UX plan in origin". |
